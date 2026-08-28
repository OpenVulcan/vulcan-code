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

/// Rebuilds the active execution request after durably promoting one pending exact session route.
/// 在可靠提升一条待生效精确会话路由后重建当前执行请求。
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
    let selection = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, request.session_id, active_stream_task_id) {
            return Err(String::from(
                "Pending Provider route activation lost exact active-stream ownership.",
            ));
        }
        let runtime_state = guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&request.session_id))
            .ok_or_else(|| {
                String::from("Pending Provider route candidate has no session runtime state.")
            })?;
        crate::conversation_submission::validated_pending_model_selection_for_provider_request(
            agent_runtime.public_ai_runtime.as_ref(),
            runtime_state,
        )?
    };
    // providerRouteLeaseIdentity is empty because preparation must capture a lease for the new exact route.
    // providerRouteLeaseIdentity 为空，因为准备阶段必须为新的精确路由捕获租约。
    let provider_route_lease_identity = None;

    // leaseScope identifies the old process-local route that must disappear in the same synchronous
    // publication that replaces the retained session tuple.
    // leaseScope 标识必须在替换保留会话三元组的同一同步发布中消失的旧进程内路由。
    let lease_scope = request.session_id.to_string();
    let lease_reconciliation = agent_runtime
        .public_ai_runtime
        .acquire_session_selection_lease_reconciliation_guard(
            std::slice::from_ref(&lease_scope),
            &[],
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
        if retained_runtime_state.pending_model_selection.as_ref() != Some(&selection) {
            return Err(String::from(
                "Pending Provider route activation no longer matches the retained exact selection.",
            ));
        }
        retained_runtime_state.active_provider_id = Some(selection.provider_id.clone());
        retained_runtime_state.active_connection_id = Some(selection.connection_id.clone());
        retained_runtime_state.active_model_id = Some(selection.model_id.clone());
        retained_runtime_state.pending_model_selection = None;
        retained_runtime_state.provider_route_lease = None;
        retained_runtime_state.updated_at = current_unix_millis_text();
    }
    // commit makes old-lease removal permanent after the exact transient mirror is published.
    // commit 在精确临时镜像发布后永久提交旧租约移除。
    lease_reconciliation.commit();

    // refreshed starts from the same logical turn but removes all inputs already represented by the canonical checkpoint.
    // refreshed 从同一逻辑回合开始，但移除已由规范检查点表示的全部输入。
    let mut refreshed = request.clone();
    refreshed.provider_id_override = Some(selection.provider_id);
    refreshed.connection_id_override = Some(selection.connection_id);
    refreshed.model_id_override = Some(selection.model_id);
    refreshed.provider_route_lease_identity = provider_route_lease_identity;
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
