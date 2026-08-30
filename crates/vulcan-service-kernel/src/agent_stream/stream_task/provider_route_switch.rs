//! Pending exact Provider-route activation at a replay-safe stream boundary.
//! 在可回放安全流边界激活待生效精确 Provider 路由。

use super::*;
use vulcan_agent_runtime::agent_runtime::execution_models::ExecutionProviderRequestContinuation;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;

/// Returns whether a durably promoted route may be published after the source stream changed ownership.
/// 判断源流所有权变化后，已可靠提升的路由是否仍可发布。
///
/// # Parameters
/// 参数
/// `retained_task_id` is the current active task when one exists; `source_task_id` is the stream that
/// reached the safe route-switch boundary.
/// `retained_task_id` 是当前存在的活动任务；`source_task_id` 是到达安全路由切换边界的源流。
///
/// # Returns
/// 返回值
/// `true` for the same owner or no owner after a concurrent stop; a replacement owner fails closed.
/// 同一所有者或并发停止后无所有者时返回 `true`；存在替代所有者时失败关闭。
fn pending_route_publication_owner_is_admissible(
    retained_task_id: Option<Uuid>,
    source_task_id: Uuid,
) -> bool {
    retained_task_id.is_none_or(|task_id| task_id == source_task_id)
}

/// Rebuilds an undispatched request from the durable session configuration after a picker change.
/// 选择器变更后，从可靠会话配置重建尚未派发的请求。
///
/// # Parameters
/// 参数
/// `task_state` owns the transient UI route mirror, `agent_runtime` validates the installed route,
/// `active_stream_task_id` proves ownership,
/// `request` is the frozen source request, `conversation_state` is the safe tool-boundary state,
/// and `provider_request_continuation` is the telemetry-owned identity reserved for the replacement stream.
/// `task_state` 持有临时 UI 路由镜像，`agent_runtime` 校验已安装路由，
/// `active_stream_task_id` 证明所有权，`request` 是冻结源请求，`conversation_state` 是安全工具边界状态，
/// `provider_request_continuation` 是为替代流保留的遥测所有身份。
///
/// # Returns
/// 返回值
/// A restored-checkpoint candidate bound to the exact selected route. The V1 Provider permit remains
/// the only owner that promotes the durable request configuration and releases network dispatch.
/// 返回绑定精确所选路由的恢复检查点候选；V1 Provider permit 仍是提升可靠请求配置并放行网络派发的唯一 owner。
pub(super) async fn refreshed_execution_request_after_pending_provider_route_switch(
    task_state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    conversation_state: SessionConversationState,
    provider_request_continuation: ExecutionProviderRequestContinuation,
) -> Result<ExecutionPreparationRequest, String> {
    // configurationTransaction serializes route promotion with every model-picker and configuration mutation.
    // configurationTransaction 将路由提升与全部模型选择器及配置变更串行化。
    let _configuration_transaction = begin_public_ai_configuration_transaction().await;
    let mut runtime_state = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, request.session_id, active_stream_task_id) {
            return Err(String::from(
                "Pending Provider route activation lost exact active-stream ownership.",
            ));
        }
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&request.session_id))
            .ok_or_else(|| {
                String::from("Pending Provider route candidate has no session runtime state.")
            })?
            .clone()
    };
    // Use the same authority as initial dispatch, not an independently promoted workspace mirror.
    // 与首次分发使用同一权威，不能读取被单独提升的工作区镜像。
    let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
        task_state,
        request.session_id,
    )?;
    crate::session_request_config_v1::synchronize_v1_dispatch_model_selection(
        handle.as_ref(),
        agent_runtime.public_ai_runtime.as_ref(),
        &mut runtime_state,
    )?;
    {
        // retainedPublication changes only the transient route mirror. Durable promotion happens later
        // inside the V1 Provider permit before the replacement request can reach the network.
        // retainedPublication 只修改临时路由镜像；可靠提升随后在 V1 Provider permit 内完成，
        // 并且早于替代请求接触网络。
        let mut guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // retainedTaskId may be absent after a concurrent user stop, but a replacement task must never
        // inherit a route transition owned by the old stream.
        // retainedTaskId 在用户并发停止后可以为空；但替代任务绝不能继承旧流拥有的路由迁移。
        let retained_task_id = guard
            .active_stream_tasks
            .get(&request.session_id)
            .map(|entry| entry.task_id());
        if !pending_route_publication_owner_is_admissible(retained_task_id, active_stream_task_id) {
            return Err(String::from(
                "Pending Provider route activation was superseded by a replacement active stream after persistence.",
            ));
        }
        let retained_runtime_state = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&request.session_id))
            .ok_or_else(|| {
                String::from(
                    "Workspace snapshot is not ready for pending Provider route activation.",
                )
            })?;
        retained_runtime_state.active_provider_id = runtime_state.active_provider_id.clone();
        retained_runtime_state.active_connection_id = runtime_state.active_connection_id.clone();
        retained_runtime_state.active_model_id = runtime_state.active_model_id.clone();
        retained_runtime_state.pending_model_selection = None;
        retained_runtime_state.provider_route_lease = runtime_state.provider_route_lease.clone();
        retained_runtime_state.active_reasoning_effort = runtime_state.active_reasoning_effort;
        retained_runtime_state.active_temperature = runtime_state.active_temperature;
        retained_runtime_state.updated_at = current_unix_millis_text();
    }
    // refreshed starts from the same logical turn but removes all inputs already represented by the canonical checkpoint.
    // refreshed 从同一逻辑回合开始，但移除已由规范检查点表示的全部输入。
    let mut refreshed = request.clone();
    refreshed.provider_id_override = runtime_state.active_provider_id;
    refreshed.connection_id_override = runtime_state.active_connection_id;
    refreshed.model_id_override = runtime_state.active_model_id;
    refreshed.provider_route_lease_identity = runtime_state.provider_route_lease;
    refreshed.reasoning_effort = runtime_state
        .active_reasoning_effort
        .unwrap_or(vulcan_provider_config::config::ReasoningEffortLevel::Off);
    refreshed.temperature = runtime_state.active_temperature;
    refreshed
        .replace_retry_replay_with_provider_request_continuation(provider_request_continuation);
    refreshed.conversation_state = conversation_state;
    refreshed.turn_start_mode = Some(
        vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint,
    );
    refreshed.provider_visible_agent_messages.clear();
    // Model-specific context and usage facts must be recomputed from the replacement route.
    // 模型专属上下文与用量事实必须根据替代路由重新计算。
    refreshed.context_compaction_policy = None;
    refreshed.context_budget_override = None;
    refreshed.context_window_prefill_input_tokens = None;
    Ok(refreshed)
}

#[cfg(test)]
mod tests {
    //! Tests for post-persistence Provider-route publication ownership.
    //! Provider 路由持久化后发布所有权测试。

    use super::*;

    /// Verifies a concurrent stop remains publishable while a replacement stream fails closed.
    /// 验证并发停止仍允许发布，而替代流会失败关闭。
    #[test]
    fn pending_route_publication_accepts_stop_but_rejects_replacement_owner() {
        // sourceTaskId identifies the stream that durably promoted the selected route.
        // sourceTaskId 标识可靠提升所选路由的源流。
        let source_task_id = Uuid::new_v4();

        assert!(pending_route_publication_owner_is_admissible(
            Some(source_task_id),
            source_task_id,
        ));
        assert!(pending_route_publication_owner_is_admissible(
            None,
            source_task_id,
        ));
        assert!(!pending_route_publication_owner_is_admissible(
            Some(Uuid::new_v4()),
            source_task_id,
        ));
    }
}
