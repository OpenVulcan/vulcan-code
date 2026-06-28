//! Prepared stream event handling helpers.
//! Prepared 流事件处理辅助逻辑。

use super::*;

/// Control-flow action returned after handling one prepared stream event.
/// 处理单个 Prepared 流事件后返回的控制流动作。
pub(crate) enum PreparedStreamEventAction {
    /// Continue polling the current provider stream.
    /// 继续轮询当前 Provider 流。
    Continue,
    /// The turn restarted with an automatically compacted request.
    /// 当前回合已经使用自动压缩后的请求重启。
    Restarted,
    /// The turn failed while preparing the provider request.
    /// 当前回合在准备 Provider 请求时失败。
    Failed,
}

/// Applies one prepared event and performs any required automatic compaction restart.
/// 应用单个 Prepared 事件，并执行必要的自动压缩重启。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and session phase updates.
/// `app` 发送工作台快照与会话阶段更新。
/// `task_state` owns active stream task state for the current WebUI session.
/// `task_state` 持有当前 WebUI 会话的活跃流任务状态。
/// `controller_runtime` persists preparation checkpoints and terminal state.
/// `controller_runtime` 持久化准备检查点与终态状态。
/// `agent_runtime` provides runtimes needed to rebuild a compacted request.
/// `agent_runtime` 提供重建压缩请求所需的运行时集合。
/// `request` is the current execution request being streamed.
/// `request` 是当前正在流式执行的请求。
/// `preparation` is the runtime preparation payload from the agent stream.
/// `preparation` 是来自 Agent 流的运行时准备载荷。
/// `session_id` identifies the active WebUI session.
/// `session_id` 标识当前活跃 WebUI 会话。
/// `stream_task_started_at` is used for latency tracing.
/// `stream_task_started_at` 用于延迟追踪。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops.
/// `auto_compaction_already_attempted` 防止重复进入自动压缩循环。
///
/// # Returns
/// 返回值
/// A control-flow action telling the caller whether to continue or stop the current task.
/// 返回控制流动作，告知调用方继续当前任务还是停止当前任务。
pub(crate) async fn handle_prepared_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    request: &ExecutionPreparationRequest,
    preparation: ExecutionPreparation,
    session_id: SessionId,
    stream_task_started_at: i128,
    auto_compaction_already_attempted: bool,
) -> PreparedStreamEventAction {
    webui_trace(
        "stream_prepared",
        format!(
            "session={} provider={} model={} elapsed_ms={}",
            preparation.session_id,
            preparation.model_blueprint.provider_id,
            preparation.model_blueprint.model_id,
            current_unix_millis_value().saturating_sub(stream_task_started_at)
        ),
    );
    if let Err(error) =
        apply_execution_preparation(app, task_state, controller_runtime, &preparation)
    {
        eprintln!("[webui-service] apply preparation failed: {error}");
    }
    let auto_compaction_result = if auto_compaction_already_attempted {
        Ok(None)
    } else {
        maybe_auto_compact_webui_session_before_provider_request(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            request,
            &preparation,
        )
        .await
    };
    match auto_compaction_result {
        Ok(Some(updated_request)) => {
            webui_trace(
                "stream_auto_compaction_restarting",
                format!("session={session_id}"),
            );
            if let Ok(mut guard) = task_state.lock() {
                guard.active_stream_tasks.remove(&session_id);
            }
            spawn_agent_stream_task_with_auto_compaction_state(
                app.clone(),
                task_state.clone(),
                controller_runtime.clone(),
                agent_runtime.clone(),
                updated_request,
                true,
            );
            PreparedStreamEventAction::Restarted
        }
        Ok(None) => PreparedStreamEventAction::Continue,
        Err(error) => {
            eprintln!(
                "[webui-service] automatic context compaction failed before provider request: {error}"
            );
            if let Ok(mut guard) = task_state.lock() {
                guard.active_stream_tasks.remove(&session_id);
                finish_webui_session_kernel_turn(
                    &mut guard,
                    session_id,
                    TurnTerminalStatus::Failed,
                );
            }
            let _ = apply_session_phase(app, task_state, session_id, SessionRunPhase::Failed);
            PreparedStreamEventAction::Failed
        }
    }
}
