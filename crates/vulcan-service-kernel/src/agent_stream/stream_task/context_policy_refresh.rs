//! Replay-safe execution refresh after a live context-compaction node change.
//! 实时切换上下文压缩节点后的可回放安全执行刷新。

use super::*;
use vulcan_agent_runtime::agent_runtime::execution_models::ExecutionProviderRequestContinuation;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;

/// Rebuilds the active execution request with the latest retained session compaction node.
/// 使用最新保留的会话压缩节点重建当前执行请求。
///
/// # Parameters
/// 参数
/// `task_state` owns the authoritative session state, `active_stream_task_id` proves exact stream
/// ownership, `request` is the frozen source request, `conversation_state` is the replay-safe tool
/// boundary, and `provider_request_continuation` preserves physical-request telemetry identity.
/// `task_state` 持有权威会话状态，`active_stream_task_id` 证明精确流所有权，`request` 是冻结源请求，
/// `conversation_state` 是可回放安全工具边界，`provider_request_continuation` 保留真实请求遥测身份。
///
/// # Returns
/// 返回值
/// A restored-checkpoint request whose policy and budget inputs will be resolved from the latest node.
/// 返回一个恢复检查点请求，其策略与预算输入将根据最新节点重新解析。
pub(super) fn refreshed_execution_request_after_context_compaction_policy_change(
    task_state: &SharedServiceBridgeState,
    active_stream_task_id: Uuid,
    request: &ExecutionPreparationRequest,
    conversation_state: SessionConversationState,
    provider_request_continuation: ExecutionProviderRequestContinuation,
) -> Result<ExecutionPreparationRequest, String> {
    let registry = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, request.session_id, active_stream_task_id) {
            return Err(String::from(
                "Context compaction policy refresh lost exact active-stream ownership.",
            ));
        }
        Arc::clone(&guard.session_runtime_registry)
    };
    let handle = registry
        .get(request.session_id)
        .map_err(|error| format!("Context compaction policy refresh has no V1 owner: {error}"))?;
    let retained_node_id = handle
        .read(|document| document.metadata.config.context_compaction_window_node_id)
        .map_err(|error| {
            format!("Context compaction policy refresh cannot read V1 metadata: {error}")
        })?;
    let next_node_id = next_context_compaction_window_node_at_safe_boundary(
        request.context_compaction_window_node_id,
        retained_node_id,
    )?;

    // refreshed reuses the same logical turn and exact route while replacing every node-derived budget fact.
    // refreshed 复用同一逻辑回合与精确路由，同时替换全部由节点派生的预算事实。
    let mut refreshed = request.clone();
    refreshed
        .replace_retry_replay_with_provider_request_continuation(provider_request_continuation);
    refreshed.conversation_state = conversation_state;
    refreshed.turn_start_mode = Some(
        vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint,
    );
    refreshed.provider_visible_agent_messages.clear();
    refreshed.context_compaction_window_node_id = next_node_id;
    refreshed.context_compaction_policy = None;
    refreshed.context_budget_override = None;
    refreshed.context_window_prefill_input_tokens = None;
    Ok(refreshed)
}

/// Resolves a changed compaction node only at the request-restart boundary.
/// 仅在请求重启边界解析已经变化的压缩节点。
///
/// # Parameters
/// 参数
/// `frozen_node_id` belongs to the active physical request, while `retained_node_id` is the
/// metadata value selected for the replacement request.
/// `frozen_node_id` 属于活动物理请求，`retained_node_id` 是为替代请求选择的 metadata 值。
///
/// # Returns
/// 返回值
/// Returns the node for the next request, or fails when no boundary change exists.
/// 返回下一次请求使用的节点；不存在边界变化时返回失败。
fn next_context_compaction_window_node_at_safe_boundary(
    frozen_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
    retained_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
) -> Result<Option<vulcan_context::ContextCompactionWindowNodeId>, String> {
    if retained_node_id == frozen_node_id {
        return Err(String::from(
            "Context compaction policy refresh reached the boundary without a node change.",
        ));
    }
    Ok(retained_node_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A live request keeps its frozen node while the replacement request receives the new node.
    /// 活动请求保留冻结节点，替代请求取得新节点。
    #[test]
    fn context_window_change_applies_only_to_the_next_safe_request_boundary() {
        let frozen_node_id = Some(
            "compression-window-1"
                .parse::<vulcan_context::ContextCompactionWindowNodeId>()
                .expect("valid frozen node"),
        );
        let retained_node_id = Some(
            "compression-window-4"
                .parse::<vulcan_context::ContextCompactionWindowNodeId>()
                .expect("valid retained node"),
        );

        let next_node_id =
            next_context_compaction_window_node_at_safe_boundary(frozen_node_id, retained_node_id)
                .expect("changed node should be admitted at the safe boundary");

        assert_eq!(
            frozen_node_id.expect("frozen node").as_str(),
            "compression-window-1"
        );
        assert_eq!(
            next_node_id.expect("next node").as_str(),
            "compression-window-4"
        );
    }

    /// Reaching the boundary without a node change is rejected explicitly.
    /// 未发生节点变化却到达刷新边界时会被显式拒绝。
    #[test]
    fn unchanged_context_window_does_not_restart_the_request() {
        let node_id = Some(
            "compression-window-4"
                .parse::<vulcan_context::ContextCompactionWindowNodeId>()
                .expect("valid context-window node"),
        );

        let error = next_context_compaction_window_node_at_safe_boundary(node_id, node_id)
            .expect_err("unchanged node must not restart the request");

        assert!(error.contains("without a node change"));
    }
}
