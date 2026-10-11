//! Session goal WebUI service handlers.
//! 会话目标 WebUI 服务处理器。

use std::fmt;
use std::sync::Arc;

use super::*;
use async_trait::async_trait;
use vulcan_agent_runtime::tooling::native::{
    NativeGoalRuntime, NativeGoalSnapshot, NativeGoalStatus, NativeGoalTerminalStatus,
};
use vulcan_service_api::{SessionGoalStatus, SessionGoalSummary};
use vulcan_session_core::v1::config::GoalState;

use crate::session_goal_lifecycle::{
    current_v1_goal_unix_millis, registered_session_v1_handle_from_state,
    session_goal_summary_from_v1_handle, session_v1_handle_from_state,
};
use crate::session_runtime_registry_v1::{
    SessionConditionalMutationReceipt, SessionFlushReason, SessionHandle, SessionMutationAdmission,
    SessionMutationChange, SessionRuntimeError,
};

mod activation;
use activation::{
    activate_user_goal_transition, resume_session_goal_v1, update_session_goal_status_v1,
};

/// Provider-visible semantic recorded when a user pauses autonomous Goal work.
/// 用户暂停目标自动执行时记录的 Provider 可见语义。
const GOAL_PAUSED_PROVIDER_CONTEXT: &str =
    "The session goal is paused. Do not continue autonomous goal work until it is resumed.";

/// Provider-visible semantic recorded when a user removes the Goal entirely.
/// 用户彻底移除目标时记录的 Provider 可见语义。
const GOAL_CLEARED_PROVIDER_CONTEXT: &str =
    "The session goal was cleared. Stop applying the previous goal context.";

/// Stable display reason recorded when the model reports that Goal work is blocked.
/// 模型报告 Goal 工作受阻时记录的稳定显示原因。
const GOAL_BLOCKED_REASON_CODE: &str = "goal_blocked";

/// Service-owned adapter exposing the authoritative V1 Goal document to native tools.
/// 服务层持有的适配器，用于向原生工具暴露权威 V1 Goal 文档。
#[derive(Clone)]
struct SessionV1NativeGoalRuntime {
    /// Publishes Goal state only after the authoritative mutation is durable.
    /// 仅在权威修改可靠落盘后发布 Goal 状态。
    app: ServiceEventEmitter,
    /// Shared service state used only to resolve the exact registered V1 session handle.
    /// 仅用于解析精确已注册 V1 会话句柄的共享服务状态。
    state: SharedServiceBridgeState,
}

impl fmt::Debug for SessionV1NativeGoalRuntime {
    /// Renders a non-sensitive adapter label without exposing the shared service state.
    /// 渲染不暴露共享服务状态的非敏感适配器标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionV1NativeGoalRuntime")
            .finish_non_exhaustive()
    }
}

/// Builds the strong V1 Goal domain port injected into one Provider execution request.
/// 构建注入单次 Provider 执行请求的强类型 V1 Goal 领域端口。
///
/// # Parameters
/// 参数
/// `state` owns the exact session registry and the temporary WebUI projection cache.
/// `state` 持有精确会话注册表与临时 WebUI 投影缓存。
/// `app` delivers the committed state to connected clients without a new user prompt.
/// `app` 无需新用户提示词即可向已连接客户端传递已提交状态。
///
/// # Returns
/// 返回值
/// A request-scoped domain port whose reads and writes use only the V1 session handle.
/// 返回仅通过 V1 会话句柄读写的请求级领域端口。
pub(crate) fn session_v1_native_goal_runtime(
    state: SharedServiceBridgeState,
    app: ServiceEventEmitter,
) -> Arc<dyn NativeGoalRuntime> {
    Arc::new(SessionV1NativeGoalRuntime { state, app })
}

#[async_trait]
impl NativeGoalRuntime for SessionV1NativeGoalRuntime {
    /// Reads one Goal from the authoritative V1 document.
    /// 从权威 V1 文档读取一个 Goal。
    async fn get_goal(&self, session_id: SessionId) -> Result<Option<NativeGoalSnapshot>, String> {
        let handle = registered_session_v1_handle_from_state(&self.state, session_id)?;
        let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
        Ok(summary.map(|summary| native_goal_snapshot_from_summary(session_id, summary)))
    }

    /// Creates a Goal or replaces only a completed generation in one durable V1 mutation.
    /// 通过一次可靠 V1 修改创建 Goal，或仅替换已经完成的代际。
    async fn create_goal(
        &self,
        session_id: SessionId,
        objective: String,
        token_budget: Option<u64>,
    ) -> Result<NativeGoalSnapshot, String> {
        let handle = registered_session_v1_handle_from_state(&self.state, session_id)?;
        let now_ms = current_v1_goal_unix_millis()?;
        // Capture the baseline only after creation succeeds, in the same durable transaction.
        // 仅在创建成功后，于同一持久化事务中记录基线。
        let usage_handle = handle.clone();
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                move |document| {
                    let transition = if document.metadata.mode.goal.enabled
                        && document.metadata.mode.goal.state == GoalState::Completed
                    {
                        document.save_goal(
                            objective.clone(),
                            token_budget,
                            objective.clone(),
                            now_ms,
                        )?
                    } else {
                        Some(document.create_goal(
                            objective.clone(),
                            token_budget,
                            objective.clone(),
                            now_ms,
                        )?)
                    };
                    document.metadata.mode.goal.usage_baseline =
                        usage_handle.goal_usage_snapshot().ok();
                    Ok(SessionMutationChange::Changed(
                        transition.is_some_and(|value| value.continuation_turn_id.is_some()),
                    ))
                },
            )
            .map_err(|error| format!("Failed to create the goal: {error}"))?;
        native_goal_snapshot_after_mutation(&self.app, &self.state, handle.as_ref(), session_id)
    }

    /// Applies a model-requested terminal Goal transition through one durable V1 mutation.
    /// 通过一次可靠 V1 修改应用模型请求的 Goal 终态转换。
    async fn update_goal_status(
        &self,
        session_id: SessionId,
        status: NativeGoalTerminalStatus,
    ) -> Result<NativeGoalSnapshot, String> {
        let handle = registered_session_v1_handle_from_state(&self.state, session_id)?;
        let now_ms = current_v1_goal_unix_millis()?;
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                move |document| match status {
                    NativeGoalTerminalStatus::Complete
                        if document.metadata.mode.goal.enabled
                            && document.metadata.mode.goal.state == GoalState::Completed =>
                    {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                    NativeGoalTerminalStatus::Complete => {
                        document.complete_goal(now_ms)?;
                        Ok(SessionMutationChange::Changed(()))
                    }
                    NativeGoalTerminalStatus::Blocked
                        if document.metadata.mode.goal.enabled
                            && document.metadata.mode.goal.state == GoalState::Blocked =>
                    {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                    NativeGoalTerminalStatus::Blocked => {
                        document.block_goal(GOAL_BLOCKED_REASON_CODE, now_ms)?;
                        Ok(SessionMutationChange::Changed(()))
                    }
                },
            )
            .map_err(|error| format!("Failed to update the goal status: {error}"))?;
        native_goal_snapshot_after_mutation(&self.app, &self.state, handle.as_ref(), session_id)
    }
}

/// Reads and caches the exact Goal projection after one successful durable mutation.
/// 在一次可靠修改成功后读取并缓存精确 Goal 投影。
///
/// # Parameters
/// 参数
/// `state` owns the temporary UI cache, `handle` owns the V1 document, and `session_id` identifies the Goal owner.
/// `state` 持有临时 UI 缓存，`handle` 持有 V1 文档，`session_id` 标识 Goal 所有者。
/// `app` broadcasts the committed Goal projection.
/// `app` 广播已提交的 Goal 投影。
///
/// # Returns
/// 返回值
/// The required native Goal snapshot, or an explicit projection error.
/// 返回必需的原生 Goal 快照，或显式投影错误。
fn native_goal_snapshot_after_mutation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    handle: &SessionHandle,
    session_id: SessionId,
) -> Result<NativeGoalSnapshot, String> {
    let summary = session_goal_summary_from_v1_handle(handle, session_id)?.ok_or_else(|| {
        format!("The goal summary of session `{session_id}` is missing after a successful update.")
    })?;
    refresh_session_goal_summary_and_emit(app, state, session_id, Some(summary.clone()))?;
    Ok(native_goal_snapshot_from_summary(session_id, summary))
}

/// Converts the service projection into the strong native-tool Goal snapshot.
/// 将服务投影转换为原生工具使用的强类型 Goal 快照。
///
/// # Parameters
/// 参数
/// `session_id` is the already validated owner and `summary` contains V1-derived Goal facts.
/// `session_id` 是已校验的所有者，`summary` 包含从 V1 派生的 Goal 事实。
///
/// # Returns
/// 返回值
/// A lossless native-tool snapshot using the same accounting totals.
/// 返回使用相同记账汇总的无损原生工具快照。
fn native_goal_snapshot_from_summary(
    session_id: SessionId,
    summary: SessionGoalSummary,
) -> NativeGoalSnapshot {
    NativeGoalSnapshot {
        usage_known: summary.usage_known,
        session_id,
        goal_id: summary.goal_id,
        objective: summary.objective,
        status: native_goal_status_from_service(summary.status),
        token_budget: summary.token_budget,
        tokens_used: summary.tokens_used,
        uncached_input_tokens: summary.input_tokens,
        cached_input_tokens: summary.cached_input_tokens,
        output_tokens: summary.output_tokens,
        reasoning_tokens: summary.reasoning_tokens,
        total_tokens: summary.total_tokens,
        time_used_seconds: summary.time_used_seconds,
        created_at_ms: summary.created_at_ms,
        updated_at_ms: summary.updated_at_ms,
    }
}

/// Maps the stable service status into the native-tool protocol status.
/// 将稳定服务状态映射为原生工具协议状态。
const fn native_goal_status_from_service(status: SessionGoalStatus) -> NativeGoalStatus {
    match status {
        SessionGoalStatus::Active => NativeGoalStatus::Active,
        SessionGoalStatus::Paused => NativeGoalStatus::Paused,
        SessionGoalStatus::Blocked => NativeGoalStatus::Blocked,
        SessionGoalStatus::UsageLimited => NativeGoalStatus::UsageLimited,
        SessionGoalStatus::BudgetLimited => NativeGoalStatus::BudgetLimited,
        SessionGoalStatus::Complete => NativeGoalStatus::Complete,
    }
}

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
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    objective: &str,
    token_budget: Option<i64>,
) -> Result<(), String> {
    let objective = normalize_session_goal_objective(objective)?;
    validate_session_goal_budget(token_budget)?;
    let handle = session_v1_handle_from_state(state, session_id).await?;
    let token_budget = token_budget.map(|value| value as u64);
    let now_ms = current_v1_goal_unix_millis()?;
    let receipt = save_session_goal_v1(handle.as_ref(), objective, token_budget, now_ms)
        .map_err(|error| format!("保存初始 V1 Goal 失败：{error}"))?;
    if !receipt.changed {
        return Err(format!("新会话 `{session_id}` 的初始 Goal 未产生 V1 修改"));
    }
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    cache_session_goal_summary(state, session_id, summary)?;
    Ok(())
}

impl SessionGoalUserStatusUpdate {
    /// Converts one user-authorized target into the V1 metadata state.
    /// 将用户授权的目标转换为 V1 metadata 状态。
    const fn into_goal_state(self) -> GoalState {
        match self {
            Self::Active => GoalState::Active,
            Self::Paused => GoalState::Paused,
        }
    }
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

/// Saves one Goal through the single failure-atomic V1 domain transition.
/// 通过唯一的失败原子 V1 领域转换保存一个 Goal。
///
/// # Parameters
/// 参数
/// `handle` owns the authoritative session document; the remaining values are validated Goal facts.
/// `handle` 持有权威会话文档；其余值为已经校验的 Goal 事实。
///
/// # Returns
/// 返回值
/// Returns the durable conditional receipt and whether a newly opened continuation Turn needs dispatch.
/// 返回可靠条件修改回执，以及是否需要调度新打开的 continuation Turn。
fn save_session_goal_v1(
    handle: &SessionHandle,
    objective: String,
    token_budget: Option<u64>,
    now_ms: i64,
) -> Result<SessionConditionalMutationReceipt<bool>, SessionRuntimeError> {
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        move |document| {
            document.apply_atomic_transition(move |document| {
                // Creation may grant a new user-owned turn; editing an existing Goal never grants execution consent.
                // 创建可以授予新的用户所属回合；编辑已有 Goal 绝不授予执行许可。
                let previous_origin = document.metadata.mode.goal.origin_seq;
                let transition =
                    document.save_goal(objective.clone(), token_budget, objective, now_ms)?;
                if document.metadata.mode.goal.origin_seq != previous_origin {
                    document.metadata.mode.goal.usage_baseline = handle.goal_usage_snapshot().ok();
                }
                Ok(match transition {
                    Some(transition) => {
                        if transition.continuation_turn_id.is_some() {
                            activate_user_goal_transition(document, &transition, now_ms)?;
                        }
                        SessionMutationChange::Changed(transition.continuation_turn_id.is_some())
                    }
                    None => SessionMutationChange::Unchanged(false),
                })
            })
        },
    )
}

/// Saves one session goal and emits an addon-state delta.
/// 保存单个会话目标并发出附属状态增量。
pub(super) async fn service_save_session_goal(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionGoalSaveRequest,
) -> Result<SessionAddonsChangedResponse, String> {
    let (controller_runtime, agent_runtime, _luaskills_catalog, _mcp_catalog, _logs) =
        runtime_parts_from_state(&state)?;
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    let objective = normalize_session_goal_objective(&request.objective)?;
    validate_session_goal_budget(request.token_budget)?;
    ensure_regular_session_exists(&state, session_id, "保存目标")?;
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "保存目标")?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let token_budget = request.token_budget.map(|value| value as u64);
    let now_ms = current_v1_goal_unix_millis()?;
    let receipt = save_session_goal_v1(handle.as_ref(), objective, token_budget, now_ms)
        .map_err(|error| format!("保存 V1 Goal 失败：{error}"))?;
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    cache_session_goal_summary(&state, session_id, summary.clone())?;
    drop(goal_transition_guard);
    let response = refresh_session_goal_summary_and_emit(&app, &state, session_id, summary)?;
    if receipt.value
        && let Err(error) = maybe_start_session_goal_continuation_if_idle(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
        )
        .await
    {
        tracing::warn!("[webui-service] created goal continuation failed: {error}");
    }
    Ok(response)
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
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "继续目标")?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let token_budget = request.token_budget.map(|value| value as u64);
    let now_ms = current_v1_goal_unix_millis()?;
    resume_session_goal_v1(handle.as_ref(), objective, token_budget, now_ms)
        .map_err(|error| format!("继续 V1 Goal 失败：{error}"))?;
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    cache_session_goal_summary(&state, session_id, summary.clone())?;
    drop(goal_transition_guard);
    let response = refresh_session_goal_summary_and_emit(&app, &state, session_id, summary)?;
    if let Err(error) = maybe_start_session_goal_continuation_if_idle(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        session_id,
    )
    .await
    {
        tracing::warn!("[webui-service] resumed goal continuation failed: {error}");
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
    let session_id = session_goal_request_session_id(&state, request.conversation_id.as_deref())?;
    ensure_regular_session_exists(&state, session_id, "清理目标")?;
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "清理目标")?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let now_ms = current_v1_goal_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document.clear_goal(GOAL_CLEARED_PROVIDER_CONTEXT, now_ms)?;
                Ok(SessionMutationChange::Changed(()))
            },
        )
        .map_err(|error| format!("清理 V1 Goal 失败：{error}"))?;
    cache_session_goal_summary(&state, session_id, None)?;
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
    let target_status = request.status.into_goal_state();
    let goal_transition_guard =
        begin_session_turn_start_exclusion(&state, [session_id], "更新目标状态")?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let now_ms = current_v1_goal_unix_millis()?;
    update_session_goal_status_v1(handle.as_ref(), request.status, now_ms)
        .map_err(|error| format!("Failed to update the goal status: {error}"))?;
    let summary = session_goal_summary_from_v1_handle(handle.as_ref(), session_id)?;
    cache_session_goal_summary(&state, session_id, summary.clone())?;
    drop(goal_transition_guard);
    let response = refresh_session_goal_summary_and_emit(&app, &state, session_id, summary)?;
    // A paused undispatched Goal turn already exists; resumption must dispatch it too, not only newly created turns.
    // 暂停且未派发的 Goal 回合可能已存在；恢复也必须派发它，不能只处理新建回合。
    if target_status == GoalState::Active
        && let Err(error) = maybe_start_session_goal_continuation_if_idle(
            &app,
            &state,
            &controller_runtime,
            &agent_runtime,
            session_id,
        )
        .await
    {
        tracing::warn!("[webui-service] active goal continuation start failed: {error}");
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

        assert_eq!(active.status.into_goal_state(), GoalState::Active);
        assert_eq!(paused.status.into_goal_state(), GoalState::Paused);
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

    /// Verifies pause and clear transitions always carry explicit non-empty exit semantics.
    /// 验证暂停与清理转换始终携带明确且非空的退出语义。
    #[test]
    fn session_goal_exit_contexts_are_explicit() {
        assert!(!GOAL_PAUSED_PROVIDER_CONTEXT.trim().is_empty());
        assert!(!GOAL_CLEARED_PROVIDER_CONTEXT.trim().is_empty());
    }
}
