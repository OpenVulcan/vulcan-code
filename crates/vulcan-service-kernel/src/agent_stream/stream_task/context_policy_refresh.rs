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
    let retained_node_id = {
        let guard = task_state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !active_stream_task_matches_id(&guard, request.session_id, active_stream_task_id) {
            return Err(String::from(
                "Context compaction policy refresh lost exact active-stream ownership.",
            ));
        }
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| workspace.session_runtime_state.get(&request.session_id))
            .ok_or_else(|| {
                String::from(
                    "Context compaction policy refresh has no retained session runtime state.",
                )
            })?
            .context_compaction_window_node_id
    };
    if retained_node_id == request.context_compaction_window_node_id {
        return Err(String::from(
            "Context compaction policy refresh reached the boundary without a node change.",
        ));
    }

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
    refreshed.context_compaction_window_node_id = retained_node_id;
    refreshed.context_compaction_policy = None;
    refreshed.context_budget_override = None;
    refreshed.context_window_prefill_input_tokens = None;
    refreshed.provider_usage_budget_baseline = None;
    refreshed.provider_usage_context_generation_id = None;
    Ok(refreshed)
}
