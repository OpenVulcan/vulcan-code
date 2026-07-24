//! Root conversation checkpoint event handling.
//! 根会话检查点事件处理。

use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_core::types::ProviderUsageSnapshot;

use super::*;

/// Projects one canonical conversation checkpoint into optional live-usage views and retained session state.
/// 将单个规范会话检查点投影到可选实时用量视图与保留会话状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench updates required by the canonical checkpoint projection.
/// `app` 发送规范检查点投影所需的工作台更新。
/// `task_state` owns live context budgets, session goal summaries, and retained workspace state.
/// `task_state` 持有实时上下文预算、会话目标摘要与保留工作区状态。
/// `controller_runtime` persists canonical main-session checkpoint state.
/// `controller_runtime` 持久化规范主会话检查点状态。
/// `session_id` identifies the root provider-stream session.
/// `session_id` 标识根 Provider 流会话。
/// `conversation_state` is the canonical provider conversation state for this checkpoint.
/// `conversation_state` 是当前检查点对应的规范 Provider 会话状态。
/// `last_successful_provider_usage` is the latest successful provider usage snapshot, when available.
/// `last_successful_provider_usage` 是最近一次成功 Provider 调用的用量快照（如存在）。
/// `provider_request_count` is the runtime-confirmed safe physical provider request count.
/// `provider_request_count` 是运行时确认安全的真实 Provider 请求次数。
/// `expected_goal_id` is the sole launch-context binding allowed to update live goal usage.
/// `expected_goal_id` 是允许更新目标实时用量的唯一启动上下文绑定。
///
/// # Returns
/// # 返回值
/// This function returns no value; each projection failure is logged independently so canonical checkpoint application is always attempted.
/// 此函数无返回值；每个投影失败都会独立记录，从而保证规范检查点始终会被尝试应用。
pub(super) fn handle_conversation_checkpoint_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: SessionConversationState,
    last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    provider_request_count: usize,
) {
    // Live context budget usage is an optional projection; its failure must not block goal accounting or the canonical checkpoint.
    // 实时上下文预算用量是可选投影；其失败不得阻塞目标计量或规范检查点。
    if let Some(usage) = last_successful_provider_usage.as_ref()
        && let Err(error) = cache_live_context_budget_observed_usage(task_state, session_id, usage)
    {
        eprintln!("[webui-service] live context budget usage update failed: {error}");
    }

    // Canonical checkpoint state is authoritative and must be attempted after all optional projections regardless of their outcomes.
    // 规范检查点状态具有权威性，无论可选投影结果如何，都必须在其后尝试应用。
    if let Err(error) = apply_conversation_checkpoint(
        app,
        task_state,
        controller_runtime,
        session_id,
        conversation_state,
        last_successful_provider_usage,
        provider_request_count,
    ) {
        eprintln!("[webui-service] apply checkpoint failed: {error}");
    }
}
