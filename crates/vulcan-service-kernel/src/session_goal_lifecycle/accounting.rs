//! Session goal lifecycle accounting helpers.
//! 会话目标生命周期 accounting 辅助逻辑。

use super::*;

/// Accounts one completed provider report against the selected session goal.
/// 将单次完成的 Provider 报告计入所选会话目标。
///
/// # Parameters
/// 参数
/// `state` owns the live goal projection updated after durable accounting.
/// `state` 持有持久记账后更新的实时目标投影。
/// `controller_runtime` provides the durable goal store connection.
/// `controller_runtime` 提供持久目标存储连接。
/// `report` carries the request-bound goal identity and terminal execution facts.
/// `report` 携带请求绑定的目标身份与终态执行事实。
/// `expected_goal_id` is the latest Goal identity bound during this execution.
/// `expected_goal_id` 是当前执行期间绑定的最新目标身份。
/// `elapsed_seconds` is the elapsed wall-clock duration attributed to the run.
/// `elapsed_seconds` 是归属于本次运行的墙钟耗时。
///
/// # Returns
/// 返回值
/// Whether the visible goal projection changed, or a durable/cache update error.
/// 返回可见目标投影是否发生变化，否则返回持久化或缓存更新错误。
pub(crate) async fn account_session_goal_from_report(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    report: &ExecutionRunReport,
    expected_goal_id: Option<&str>,
    elapsed_seconds: i64,
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
    // isRegularSession prevents detached task reports from changing the root session goal.
    // isRegularSession 防止分离任务报告修改根会话目标。
    let is_regular_session = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .as_ref()
        .is_some_and(|workspace_snapshot| {
            workspace_snapshot_contains_regular_session(workspace_snapshot, report.session_id)
        });
    if !is_regular_session || (elapsed_seconds <= 0 && report.error.is_none()) {
        return Ok(false);
    }
    // projectionChanged records whether terminal time or error accounting changed the visible goal.
    // projectionChanged 记录终态耗时或错误记账是否改变了可见目标。
    let mut projection_changed = false;
    // goalStore owns durable goal accounting; token usage is already persisted per Provider request.
    // goalStore 负责持久目标记账；Token 用量已按每次 Provider 请求持久化。
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    if elapsed_seconds > 0 {
        let outcome = goal_store
            .account_thread_goal_usage(report.session_id, 0, elapsed_seconds, expected_goal_id)
            .await?;
        if let GoalAccountingOutcome::Updated(goal) = outcome {
            advance_session_goal_dispatch_version(state, report.session_id)?;
            projection_changed |=
                merge_session_goal_accounting_projection(state, report.session_id, &goal)?
                    .is_some();
        }
    }
    if let Some(error) = report.error.as_deref() {
        projection_changed |= apply_session_goal_error_status(
            state,
            &goal_store,
            report.session_id,
            expected_goal_id,
            error,
        )
        .await?;
    }
    Ok(projection_changed)
}

/// Resolves the exact goal identity that owns a newly started physical provider request.
/// 解析拥有新启动物理 Provider 请求的精确目标身份。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative in-process goal projection.
/// `state` 持有权威的进程内目标投影。
/// `session_id` identifies the regular session starting the request.
/// `session_id` 标识正在启动请求的常规会话。
/// `previously_bound_goal_id` preserves ownership across tool-result requests after the goal stops.
/// `previously_bound_goal_id` 在目标停止后跨工具结果请求保留归属关系。
///
/// # Returns
/// 返回值
/// The exact active or execution-bound goal id, no goal when none can own the request, or a lock error.
/// 返回精确的活动或执行绑定目标标识；没有目标可拥有请求时为空；锁失败时返回错误。
pub(crate) fn session_goal_id_for_provider_request(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    previously_bound_goal_id: Option<&str>,
) -> Result<Option<String>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(goal) = guard.session_goal_summaries.get(&session_id) else {
        // A request-bound goal remains safe to return because durable accounting verifies the exact id.
        // 请求绑定目标可安全返回，因为持久化记账还会校验精确标识。
        return Ok(previously_bound_goal_id.map(str::to_owned));
    };
    if goal.status == SessionGoalStatus::Active
        || previously_bound_goal_id.is_some_and(|goal_id| goal_id == goal.goal_id)
    {
        return Ok(Some(goal.goal_id.clone()));
    }
    Ok(None)
}

/// Persists one successful physical provider request and refreshes its goal projection.
/// 持久化一次成功的物理 Provider 请求并刷新其目标投影。
///
/// # Parameters
/// 参数
/// `state` owns the visible goal cache and dispatch version.
/// `state` 持有可见目标缓存与分发版本。
/// `controller_runtime` provides the durable goal store connection.
/// `controller_runtime` 提供持久目标存储连接。
/// `session_id` identifies the session that owns the provider request.
/// `session_id` 标识拥有 Provider 请求的会话。
/// `expected_goal_id` is the goal bound when the physical request started.
/// `expected_goal_id` 是物理请求启动时绑定的目标。
/// `usage` carries exact normalized Provider usage for the completed request.
/// `usage` 携带已完成请求的精确标准化 Provider 用量。
///
/// # Returns
/// 返回值
/// Whether the bound goal changed, or a durable/cache update error.
/// 返回绑定目标是否变化，否则返回持久化或缓存更新错误。
pub(crate) async fn account_session_goal_provider_usage(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    expected_goal_id: &str,
    usage: &ProviderRequestCompletedUsage,
) -> Result<bool, String> {
    // persistedUsage converts platform-sized counters into the store's saturating integer contract.
    // persistedUsage 将平台大小计数转换为存储层的饱和整数契约。
    let persisted_usage = ThreadGoalProviderUsage {
        uncached_input_tokens: i64::try_from(usage.uncached_input_tokens).unwrap_or(i64::MAX),
        cached_input_tokens: i64::try_from(usage.cached_input_tokens).unwrap_or(i64::MAX),
        output_tokens: i64::try_from(usage.output_tokens).unwrap_or(i64::MAX),
        reasoning_tokens: i64::try_from(usage.reasoning_tokens).unwrap_or(i64::MAX),
        total_tokens: i64::try_from(usage.total_tokens).unwrap_or(i64::MAX),
    };
    // goalStore applies request usage atomically against the request-bound goal identity.
    // goalStore 针对请求绑定的目标身份原子应用请求用量。
    let goal_store = VulcanGoalStore::new(controller_runtime.clone());
    let outcome = goal_store
        .account_thread_goal_provider_usage(session_id, persisted_usage, expected_goal_id)
        .await?;
    let GoalAccountingOutcome::Updated(goal) = outcome else {
        return Ok(false);
    };
    advance_session_goal_dispatch_version(state, session_id)?;
    // visibleSummary is present only when the same goal remains visible and accounting changed it.
    // visibleSummary 仅在同一目标仍然可见且记账改变了它时存在。
    let Some(visible_summary) = merge_session_goal_accounting_projection(state, session_id, &goal)?
    else {
        return Ok(false);
    };
    if visible_summary.status == SessionGoalStatus::BudgetLimited
        && let Err(error) =
            enqueue_session_goal_budget_limited_guidance(state, session_id, &visible_summary)
    {
        eprintln!("[webui-service] enqueue goal budget-limited guidance failed: {error}");
    }
    Ok(true)
}

/// Marks an active goal with the terminal status implied by a failed run.
/// 根据失败运行暗示的终态标记当前 active 目标。
///
/// # Parameters
/// 参数
/// `state` owns the live goal projection.
/// `state` 持有实时目标投影。
/// `goal_store` persists the lifecycle transition.
/// `goal_store` 持久化生命周期转换。
/// `session_id` identifies the failed run session.
/// `session_id` 标识失败运行所属会话。
/// `expected_goal_id` binds the error to the goal captured at launch.
/// `expected_goal_id` 将错误绑定到启动时捕获的目标。
/// `error` is the provider/runtime error used to choose the terminal status.
/// `error` 是用于选择终态的 Provider/运行时错误。
///
/// # Returns
/// 返回值
/// Whether the visible goal changed, or a store/cache error.
/// 返回可见目标是否发生变化，否则返回存储或缓存错误。
pub(crate) async fn apply_session_goal_error_status(
    state: &SharedServiceBridgeState,
    goal_store: &VulcanGoalStore,
    session_id: SessionId,
    expected_goal_id: &str,
    error: &str,
) -> Result<bool, String> {
    let Some(goal) = goal_store.get_thread_goal(session_id).await? else {
        return Ok(false);
    };
    if goal.goal_id != expected_goal_id {
        return Ok(false);
    }
    if goal.status != ThreadGoalStatus::Active {
        cache_session_goal_summary(state, session_id, Some(goal))?;
        return Ok(false);
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
                expected_status: Some(goal.status),
            },
        )
        .await?;
    if let Some(updated_goal) = updated_goal {
        advance_session_goal_dispatch_version(state, session_id)?;
        cache_session_goal_summary(state, session_id, Some(updated_goal))?;
        return Ok(true);
    }
    Ok(false)
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
