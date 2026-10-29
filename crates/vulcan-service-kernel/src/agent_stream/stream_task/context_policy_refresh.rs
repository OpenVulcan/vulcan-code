//! Replay-safe execution refresh after a live context-compaction node change.
//! 实时切换上下文压缩节点后的可回放安全执行刷新。

use super::*;
use vulcan_agent_runtime::agent_runtime::execution_models::ExecutionProviderRequestContinuation;
use vulcan_agent_runtime::agent_runtime::runtime_stream_intervention::{
    RuntimePromptIntervention, RuntimePromptInterventionError,
};
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_core::controller::VulcanCoreController;

/// Builds the safe-boundary intervention that stops the active request once the session node changes.
/// 构建安全边界干预器，在会话压缩节点变化后停止当前请求。
///
/// # Parameters
/// 参数
/// `state` owns the retained session runtime rows, and `frozen_node_id` is the node captured by the
/// active execution request.
/// `state` 持有保留的会话运行时行，`frozen_node_id` 是当前执行请求捕获的节点。
///
/// # Returns
/// 返回值
/// A callback that never edits the controller and requests a policy refresh on a node change.
/// 返回一个从不修改控制器、在节点变化时请求策略刷新的回调。
pub(super) fn context_compaction_node_change_intervention(
    state: SharedServiceBridgeState,
    frozen_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
) -> RuntimePromptIntervention {
    Arc::new(move |session_id, _controller: &mut VulcanCoreController| {
        let retained_node_id = {
            let guard =
                state
                    .lock()
                    .map_err(|error| RuntimePromptInterventionError::StateAccess {
                        message: format!("failed to lock service state: {error}"),
                    })?;
            let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
                RuntimePromptInterventionError::StateUnavailable {
                    message: String::from("workspace snapshot is not ready"),
                }
            })?;
            workspace
                .session_runtime_state
                .get(&session_id)
                .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
        };
        ensure_context_compaction_node_unchanged(session_id, frozen_node_id, retained_node_id)?;
        Ok(false)
    })
}

/// Rejects continuing a request whose frozen node no longer matches the retained session node.
/// 拒绝继续冻结节点已与保留会话节点不一致的请求。
///
/// # Parameters
/// 参数
/// `session_id` names the session in the diagnostic; `frozen_node_id` and `retained_node_id` are compared.
/// `session_id` 用于诊断中标识会话；比较 `frozen_node_id` 与 `retained_node_id`。
///
/// # Returns
/// 返回值
/// Success when unchanged, or the policy-refresh error that restarts the request at this boundary.
/// 未变化时成功；否则返回在此边界重启请求的策略刷新错误。
fn ensure_context_compaction_node_unchanged(
    session_id: SessionId,
    frozen_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
    retained_node_id: Option<vulcan_context::ContextCompactionWindowNodeId>,
) -> Result<(), RuntimePromptInterventionError> {
    if retained_node_id == frozen_node_id {
        return Ok(());
    }
    let label = |node_id: Option<vulcan_context::ContextCompactionWindowNodeId>| {
        node_id
            .map_or("model default", |node_id| node_id.as_str())
            .to_owned()
    };
    Err(
        RuntimePromptInterventionError::ContextCompactionPolicyRefreshRequired {
            message: format!(
                "session `{session_id}` changed context-compaction node from `{}` to `{}`",
                label(frozen_node_id),
                label(retained_node_id)
            ),
        },
    )
}

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
        let frozen_node_id = "compression-window-1"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid frozen node");
        let retained_node_id = Some(
            "compression-window-4"
                .parse::<vulcan_context::ContextCompactionWindowNodeId>()
                .expect("valid retained node"),
        );

        let next_node_id = next_context_compaction_window_node_at_safe_boundary(
            Some(frozen_node_id),
            retained_node_id,
        )
        .expect("changed node should be admitted at the safe boundary");

        assert_eq!(frozen_node_id.as_str(), "compression-window-1");
        assert_eq!(
            next_node_id.expect("next node").as_str(),
            "compression-window-4"
        );
    }

    /// A node chosen during the run stops the active request, while an unchanged node lets it continue.
    /// 运行中选定的新节点会停止当前请求，未变化的节点允许继续。
    #[test]
    fn node_change_during_run_requests_policy_refresh() {
        let session_id = SessionId::from_u128(9_441);
        let default_node = "compression-window-5"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid default node");
        let locked_node = "compression-window-3"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid locked node");

        assert!(
            ensure_context_compaction_node_unchanged(
                session_id,
                Some(locked_node),
                Some(locked_node)
            )
            .is_ok()
        );
        for (frozen, retained) in [
            (None, Some(locked_node)),
            (Some(default_node), Some(locked_node)),
            (Some(locked_node), None),
        ] {
            assert!(matches!(
                ensure_context_compaction_node_unchanged(session_id, frozen, retained),
                Err(RuntimePromptInterventionError::ContextCompactionPolicyRefreshRequired { .. })
            ));
        }
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
