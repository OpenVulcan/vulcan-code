//! Session goal WebUI service handlers.
//! 会话目标 WebUI 服务处理器。

use super::*;

/// WebUI 输入区目标弹窗发送的会话目标保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionGoalSaveRequest {
    /// Conversation id whose goal should be created or updated.
    /// 需要创建或更新目标的会话标识。
    conversation_id: Option<String>,
    /// Concrete objective text to persist.
    /// 需要持久化的具体目标文本。
    objective: String,
    /// Optional positive token budget explicitly requested by the user.
    /// 用户明确要求时提供的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Session goal resume request sent by the WebUI budget-limited goal strip.
/// WebUI 预算耗尽目标条发送的会话目标继续请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionGoalResumeRequest {
    /// Conversation id whose stopped goal should resume.
    /// 需要恢复已停止目标的会话标识。
    conversation_id: Option<String>,
    /// User-authored objective that should continue in the next stage.
    /// 下一阶段需要继续推进的用户目标文本。
    objective: String,
    /// Optional positive token budget for the next stage.
    /// 下一阶段使用的可选正数 token 预算。
    token_budget: Option<i64>,
}

/// Session goal clear request sent by the WebUI goal area.
/// WebUI 目标区域发送的会话目标清理请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionGoalClearRequest {
    /// Conversation id whose goal should be removed.
    /// 需要移除目标的会话标识。
    conversation_id: Option<String>,
}

/// User-authorized lifecycle targets accepted by the WebUI status toggle.
/// WebUI 状态切换器接受的用户授权生命周期目标。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionGoalUserStatusUpdate {
    /// Reactivates one paused goal.
    /// 重新激活一个已暂停目标。
    Active,
    /// Pauses one active goal.
    /// 暂停一个活动目标。
    Paused,
}

/// Persists a validated goal captured atomically with a newly materialized submission.
/// 持久化与新物化提交原子捕获且已校验的目标。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the durable goal store, `session_id` owns the new goal, and the remaining values are user input.
/// `controller_runtime` 持有持久目标存储，`session_id` 持有新目标，其余值为用户输入。
///
/// # Returns
/// 返回值
/// Returns success after the active goal is durably created.
/// 在活动目标持久创建后返回成功。
pub(crate) async fn persist_initial_submission_goal(
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    objective: &str,
    token_budget: Option<i64>,
) -> Result<(), String> {
    let objective = normalize_session_goal_objective(objective)?;
    validate_session_goal_budget(token_budget)?;
    VulcanGoalStore::new(controller_runtime.clone())
        .replace_thread_goal(
            session_id,
            objective.as_str(),
            ThreadGoalStatus::Active,
            token_budget,
        )
        .await
        .map_err(|error| format!("保存初始目标失败：{error}"))?;
    Ok(())
}

impl SessionGoalUserStatusUpdate {
    /// Converts one user-authorized target into the persisted store status.
    /// 将用户授权的目标转换为持久化存储状态。
    const fn into_thread_goal_status(self) -> ThreadGoalStatus {
        match self {
            Self::Active => ThreadGoalStatus::Active,
            Self::Paused => ThreadGoalStatus::Paused,
        }
    }
}

/// Validates that a user status request does not override a system-owned lifecycle state.
/// 校验用户状态请求不会覆盖系统拥有的生命周期状态。
fn ensure_session_goal_user_status_transition(
    current: ThreadGoalStatus,
    target: ThreadGoalStatus,
) -> Result<(), String> {
    if matches!(current, ThreadGoalStatus::Active | ThreadGoalStatus::Paused)
        && matches!(target, ThreadGoalStatus::Active | ThreadGoalStatus::Paused)
    {
        return Ok(());
    }
    Err(format!(
        "当前目标状态 `{}` 不允许用户直接切换为 `{}`。",
        current.as_str(),
        target.as_str()
    ))
}

/// Session goal status update request sent by the WebUI goal area.
/// WebUI 目标区域发送的会话目标状态更新请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionGoalStatusUpdateRequest {
    /// Conversation id whose goal status should change.
    /// 需要更新目标状态的会话标识。
    conversation_id: Option<String>,
    /// Next status key accepted by the user-facing controls.
    /// 用户界面控件接受的下一状态键。
    status: SessionGoalUserStatusUpdate,
}

/// Saves one session goal and emits an addon-state delta.
/// 保存单个会话目标并发出附属状态增量。
pub(super) async fn service_save_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalSaveRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    let objective = normalize_session_goal_objective(&request.objective)?;
    validate_session_goal_budget(request.token_budget)?;
    ensure_regular_session_exists(&state, session_id, "保存目标")?;
    ensure_session_not_in_plan_mode_for_goal(&state, session_id)?;
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "保存目标")?;
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?;
    let objective_changed = existing_goal
        .as_ref()
        .is_some_and(|goal| goal.objective != objective);
    let goal = if let Some(existing_goal) = existing_goal {
        if existing_goal.status == ThreadGoalStatus::Complete {
            goal_store
                .replace_thread_goal(
                    session_id,
                    objective.as_str(),
                    ThreadGoalStatus::Active,
                    request.token_budget,
                )
                .await
                .map_err(|error| format!("保存目标失败：{error}"))?
        } else {
            goal_store
                .update_thread_goal(
                    session_id,
                    GoalUpdate {
                        objective: Some(objective),
                        status: Some(saved_goal_status_after_edit(existing_goal.status)),
                        token_budget: Some(request.token_budget),
                        expected_goal_id: Some(existing_goal.goal_id),
                        expected_status: Some(existing_goal.status),
                    },
                )
                .await
                .map_err(|error| format!("保存目标失败：{error}"))?
                .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?
        }
    } else {
        goal_store
            .replace_thread_goal(
                session_id,
                objective.as_str(),
                ThreadGoalStatus::Active,
                request.token_budget,
            )
            .await
            .map_err(|error| format!("保存目标失败：{error}"))?
    };
    advance_session_goal_dispatch_version(&state, session_id)?;
    if objective_changed
        && goal.status == ThreadGoalStatus::Active
        && let Err(error) = enqueue_session_goal_objective_updated_guidance(
            &state,
            session_id,
            &session_goal_summary_from_thread_goal(&goal),
        )
    {
        eprintln!("[webui-service] enqueue goal objective update guidance failed: {error}");
    }
    drop(goal_transition_guard);
    refresh_session_goal_summary_and_emit(&app, &state, session_id, Some(goal))
}

/// Returns the lifecycle status to keep after a user edits an existing goal.
/// 返回用户编辑既有目标后应保留的生命周期状态。
pub(super) fn saved_goal_status_after_edit(status: ThreadGoalStatus) -> ThreadGoalStatus {
    if status == ThreadGoalStatus::Complete {
        ThreadGoalStatus::Active
    } else {
        status
    }
}

/// Resumes a stopped session goal as a fresh user-approved budget stage.
/// 将已停止的会话目标恢复为用户确认后的新预算阶段。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target session, objective, and next-stage budget.
/// `request` 携带目标会话、目标文本与下一阶段预算。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the goal resumes.
/// 目标恢复后的刷新工作台快照。
pub(super) async fn service_resume_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalResumeRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let (controller_runtime, agent_runtime, _luaskills_catalog, _mcp_catalog, _logs) =
        runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    let objective = normalize_session_goal_objective(&request.objective)?;
    validate_session_goal_budget(request.token_budget)?;
    ensure_regular_session_exists(&state, session_id, "继续目标")?;
    ensure_session_not_in_plan_mode_for_goal(&state, session_id)?;
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "继续目标")?;
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?
        .ok_or_else(|| String::from("当前会话还没有可继续的目标。"))?;
    if existing_goal.status == ThreadGoalStatus::Complete {
        return Err(String::from("已完成目标不能继续，请创建新目标。"));
    }
    if existing_goal.status == ThreadGoalStatus::Active {
        return Err(String::from("当前目标已经在进行中。"));
    }
    let objective_changed = existing_goal.objective != objective;
    let resumed_from_goal_id = existing_goal.goal_id.clone();
    let goal = goal_store
        .resume_thread_goal(
            session_id,
            GoalResume {
                objective,
                token_budget: request.token_budget,
                expected_goal_id: resumed_from_goal_id.clone(),
                expected_status: existing_goal.status,
            },
        )
        .await
        .map_err(|error| format!("继续目标失败：{error}"))?
        .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?;
    if goal.status != ThreadGoalStatus::Active || goal.goal_id == resumed_from_goal_id {
        return Err(String::from("目标继续结果没有创建新的活动预算阶段。"));
    }
    advance_session_goal_dispatch_version(&state, session_id)?;
    clear_session_goal_stage_runtime_state(&state, session_id, &resumed_from_goal_id)?;
    if objective_changed
        && let Err(error) = enqueue_session_goal_objective_updated_guidance(
            &state,
            session_id,
            &session_goal_summary_from_thread_goal(&goal),
        )
    {
        eprintln!("[webui-service] enqueue resumed goal objective update guidance failed: {error}");
    }
    drop(goal_transition_guard);
    let response = refresh_session_goal_summary_and_emit(&app, &state, session_id, Some(goal))?;
    if let Err(error) = maybe_start_session_goal_continuation_if_idle(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        session_id,
    )
    .await
    {
        eprintln!("[webui-service] resumed goal continuation failed: {error}");
    }
    Ok(response)
}

/// Clears the selected session goal through the user-facing WebUI API.
/// 通过面向用户的 WebUI API 清理当前会话目标。
///
/// # Parameters
/// 参数
/// `app` emits the addon delta when the target session is visible.
/// `app` 在目标会话可见时发出附属状态增量。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target conversation id.
/// `request` 携带目标会话标识。
///
/// # Returns
/// 返回值
/// The applied addon projection after the goal is removed.
/// 目标移除后的附属状态投影。
pub(super) async fn service_clear_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalClearRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    ensure_regular_session_exists(&state, session_id, "清理目标")?;
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "清理目标")?;
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?
        .ok_or_else(|| String::from("当前会话还没有可清理的目标。"))?;
    let expected_goal_id = existing_goal.goal_id;
    let removed_goal = goal_store
        .delete_thread_goal(session_id, &expected_goal_id)
        .await
        .map_err(|error| format!("清理目标失败：{error}"))?
        .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?;
    if removed_goal.goal_id != expected_goal_id {
        return Err(String::from("目标清理结果与请求不一致，请刷新后重试。"));
    }
    advance_session_goal_dispatch_version(&state, session_id)?;
    clear_session_goal_stage_runtime_state(&state, session_id, &removed_goal.goal_id)?;
    drop(goal_transition_guard);
    refresh_session_goal_summary_and_emit(&app, &state, session_id, None)
}

/// Updates the selected session goal status through the user-facing WebUI API.
/// 通过面向用户的 WebUI API 更新当前会话目标状态。
///
/// # Parameters
/// 参数
/// `app` emits the addon delta when the target session is visible.
/// `app` 在目标会话可见时发出附属状态增量。
/// `state` owns the retained service bridge state.
/// `state` 持有保留的服务桥接状态。
/// `request` carries the target conversation id and next status.
/// `request` 携带目标会话标识与下一状态。
///
/// # Returns
/// 返回值
/// The applied addon projection after the goal status changes.
/// 目标状态变更后的附属状态投影。
pub(super) async fn service_update_session_goal_status(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalStatusUpdateRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let (controller_runtime, agent_runtime, _luaskills_catalog, _mcp_catalog, _logs) =
        runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    ensure_regular_session_exists(&state, session_id, "更新目标状态")?;
    let target_status = request.status.into_thread_goal_status();
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "更新目标状态")?;
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let existing_goal = goal_store
        .get_thread_goal(session_id)
        .await
        .map_err(|error| format!("读取当前目标失败：{error}"))?
        .ok_or_else(|| String::from("当前会话还没有可更新的目标。"))?;
    ensure_session_goal_user_status_transition(existing_goal.status, target_status)?;
    let expected_goal_id = existing_goal.goal_id;
    let expected_status = existing_goal.status;
    let goal = goal_store
        .update_thread_goal(
            session_id,
            GoalUpdate {
                objective: None,
                status: Some(target_status),
                token_budget: None,
                expected_goal_id: Some(expected_goal_id.clone()),
                expected_status: Some(expected_status),
            },
        )
        .await
        .map_err(|error| format!("更新目标状态失败：{error}"))?
        .ok_or_else(|| String::from("当前目标已变化，请刷新后重试。"))?;
    if goal.goal_id != expected_goal_id || goal.status != target_status {
        return Err(String::from("目标状态更新结果与请求不一致，请刷新后重试。"));
    }
    advance_session_goal_dispatch_version(&state, session_id)?;
    if target_status == ThreadGoalStatus::Paused {
        clear_session_goal_stage_runtime_state(&state, session_id, &goal.goal_id)?;
    }
    drop(goal_transition_guard);
    let response = refresh_session_goal_summary_and_emit(&app, &state, session_id, Some(goal))?;
    if target_status == ThreadGoalStatus::Active
        && let Err(error) = maybe_start_session_goal_continuation_if_idle(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
        )
        .await
    {
        eprintln!("[webui-service] active goal continuation start failed: {error}");
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the user status request accepts only active and paused transitions.
    /// 验证用户状态请求只接受 active 与 paused 转换。
    #[test]
    fn session_goal_status_update_request_accepts_user_transitions() {
        let active: SessionGoalStatusUpdateRequest = serde_json::from_value(json!({
            "conversationId": SessionId::new_v4().to_string(),
            "status": "active"
        }))
        .expect("active user transition should deserialize");
        let paused: SessionGoalStatusUpdateRequest = serde_json::from_value(json!({
            "conversationId": SessionId::new_v4().to_string(),
            "status": "paused"
        }))
        .expect("paused user transition should deserialize");

        assert_eq!(
            active.status.into_thread_goal_status(),
            ThreadGoalStatus::Active
        );
        assert_eq!(
            paused.status.into_thread_goal_status(),
            ThreadGoalStatus::Paused
        );
    }

    /// Verifies system-owned and unknown statuses are rejected during RPC request decoding.
    /// 验证系统拥有状态与未知状态会在 RPC 请求解码阶段被拒绝。
    #[test]
    fn session_goal_status_update_request_rejects_system_statuses() {
        for status in [
            "blocked",
            "usage_limited",
            "budget_limited",
            "complete",
            "unknown",
        ] {
            let error = serde_json::from_value::<SessionGoalStatusUpdateRequest>(json!({
                "conversationId": SessionId::new_v4().to_string(),
                "status": status
            }))
            .expect_err("system-owned or unknown status must not deserialize");

            assert!(error.to_string().contains("unknown variant"));
        }
    }

    /// Verifies user transitions cannot overwrite system-owned lifecycle states.
    /// 验证用户转换不能覆盖系统拥有的生命周期状态。
    #[test]
    fn session_goal_user_status_transition_rejects_system_owned_current_state() {
        assert!(
            ensure_session_goal_user_status_transition(
                ThreadGoalStatus::Active,
                ThreadGoalStatus::Paused
            )
            .is_ok()
        );
        assert!(
            ensure_session_goal_user_status_transition(
                ThreadGoalStatus::Paused,
                ThreadGoalStatus::Active
            )
            .is_ok()
        );
        for current in [
            ThreadGoalStatus::Blocked,
            ThreadGoalStatus::UsageLimited,
            ThreadGoalStatus::BudgetLimited,
            ThreadGoalStatus::Complete,
        ] {
            assert!(
                ensure_session_goal_user_status_transition(current, ThreadGoalStatus::Active)
                    .is_err()
            );
        }
    }
}
