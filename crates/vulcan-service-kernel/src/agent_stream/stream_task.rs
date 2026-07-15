//! Agent runtime stream task entrypoints.
//! Agent 运行时流任务入口。

use super::*;

mod content_delta;
mod conversation_checkpoint;
mod finished;
mod launch_failure;
mod prepared;
mod request_phase;
mod request_trace;
mod runtime_event;
mod stream_exit;

use conversation_checkpoint::handle_conversation_checkpoint_stream_event;
use finished::{FinishedStreamEventOutcome, handle_finished_stream_event};
pub(crate) use launch_failure::StreamTaskLaunchFailureCommit;
#[cfg(test)]
pub(crate) use launch_failure::StreamTaskLaunchFailureOutcome;
pub(crate) use launch_failure::{
    commit_stream_task_registration_failure, handle_pre_registration_stream_task_launch_failure,
    handle_stream_task_registration_failure,
};
use prepared::{PreparedStreamEventAction, handle_prepared_stream_event};
#[cfg(test)]
pub(crate) use prepared::{PreparedStreamFailureCommit, commit_prepared_stream_failure};
use request_phase::{RequestPhaseStreamEvent, handle_request_phase_stream_event};
use request_trace::StreamRequestTraceState;
use runtime_event::{handle_runtime_stream_event, runtime_event_hook_policy_for_execution_scope};
#[cfg(test)]
pub(crate) use stream_exit::StreamTaskExitOutcome;
pub(crate) use stream_exit::{StreamTaskExitReason, handle_stream_task_exit};

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
pub(crate) fn spawn_agent_stream_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
) {
    spawn_agent_stream_task_with_auto_compaction_state(
        app,
        state,
        controller_runtime,
        agent_runtime,
        request,
        false,
    );
}

/// Spawns one background task that forwards agent runtime stream events with per-request auto-compaction state.
/// 启动一个携带单请求自动压缩状态的后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench snapshots and runtime stream projections.
/// `app` 发出工作台快照与运行时流投影。
/// `state` owns retained service state and active stream handles.
/// `state` 持有保留的服务状态与活跃流句柄。
/// `controller_runtime` persists provider checkpoints and terminal session state.
/// `controller_runtime` 持久化 Provider 检查点与终态会话状态。
/// `agent_runtime` provides provider, tool, hook, and compaction runtimes.
/// `agent_runtime` 提供 Provider、工具、hook 与压缩运行时。
/// `request` is the prepared execution request for the current user turn.
/// `request` 是当前用户回合的待执行请求。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops for one user request.
/// `auto_compaction_already_attempted` 防止单个用户请求反复进入自动压缩循环。
pub(crate) fn spawn_agent_stream_task_with_auto_compaction_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
    auto_compaction_already_attempted: bool,
) {
    let session_id = request.session_id;
    // expected_kernel_generation binds any pre-registration contract failure to the exact active turn.
    // expected_kernel_generation 将任意预注册契约失败绑定到精确活动回合。
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    // runtime_event_hook_policy captures the request-authoritative Hook scope before the request moves into the task.
    // runtime_event_hook_policy 在请求移入任务前捕获由请求权威确定的 Hook 范围。
    let runtime_event_hook_policy = runtime_event_hook_policy_for_execution_scope(
        &request.execution_scope,
        request.runtime_policy.hooks_enabled,
        request.conversation_hook_turn_id.as_deref(),
    );
    let runtime_event_hook_policy = match runtime_event_hook_policy {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("[webui-service] runtime event Hook correlation rejected: {error}");
            // cleanup_outcome records how the pre-registration contract failure terminalized the session.
            // cleanup_outcome 记录预注册契约失败如何终态化该会话。
            let cleanup_outcome = handle_pre_registration_stream_task_launch_failure(
                &app,
                &state,
                session_id,
                expected_kernel_generation,
            );
            match cleanup_outcome {
                Ok(outcome) => webui_trace(
                    "stream_hook_correlation_rejected",
                    format!("session={session_id} cleanup={outcome:?} error={error}"),
                ),
                Err(cleanup_error) => eprintln!(
                    "[webui-service] runtime event Hook correlation cleanup failed: {cleanup_error}; original error: {error}"
                ),
            }
            return;
        }
    };
    let session_goal_id = request
        .session_goal_context
        .as_ref()
        .map(|context| context.goal_id.clone());
    let active_stream_task_id = Uuid::new_v4();
    webui_after_precheck_trace(
        "after_precheck_19_stream_task_scheduled",
        format!("session={session_id}"),
    );
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let task_state = state.clone();
    let registration_failure_app = app.clone();
    let registration_failure_state = state.clone();
    let task_future = async move {
        let stream_task_started_at = current_unix_millis_value();
        webui_after_precheck_trace(
            "after_precheck_20_stream_task_entered",
            format!("session={session_id}"),
        );
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        // providerTelemetryGuard closes an active physical request even when this task future is aborted.
        // providerTelemetryGuard 即使当前任务 Future 被中止，也会关闭活动真实请求。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new(task_state.clone());
        let intervention_started_at = current_unix_millis_value();
        let prompt_ai_tool_configuration = request.ai_tool_configuration.clone();
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            agent_runtime.provider_configuration.clone(),
            prompt_ai_tool_configuration,
        );
        webui_after_precheck_trace(
            "after_precheck_21_intervention_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(intervention_started_at)
            ),
        );
        let provider_stream_create_started_at = current_unix_millis_value();
        webui_trace(
            "stream_provider_create_start",
            format!("session={session_id}"),
        );
        let mut stream = Box::pin(stream_runtime_run_with_intervention_and_plugins(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            Some(agent_runtime.system_plugin_runtime.clone()),
            agent_runtime.mcp_runtime.clone(),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Arc::clone(&agent_runtime.token_budget_runtime),
            request.clone(),
            Some(prompt_intervention),
        ));

        webui_trace(
            "stream_provider_create_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_22_stream_created",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_trace(
            "stream_task_poll_start",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_23_poll_start",
            format!(
                "session={session_id} elapsed_ms_since_task={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        // This request-local state machine keeps latency markers consistent across every stream event kind.
        // 该请求局部状态机用于确保所有流事件类型的延迟标记保持一致。
        let mut request_trace_state = StreamRequestTraceState::new(stream_task_started_at);
        // stream_exit_reason makes every polling-loop terminal path explicit without a shadow completion boolean.
        // stream_exit_reason 显式表达每条轮询循环终止路径，不再使用影子完成布尔值。
        let stream_exit_reason = loop {
            // event is the next ordered runtime item owned by this exact stream generation.
            // event 是当前精确流代际拥有的下一条有序运行时事件。
            let Some(event) = stream.next().await else {
                break StreamTaskExitReason::StreamExhausted;
            };
            if !webui_stream_event_matches_kernel_generation(
                &task_state,
                &event,
                expected_kernel_generation,
            ) {
                webui_trace(
                    "stream_event_skipped_stale_generation",
                    format!(
                        "session={} expected_generation={}",
                        execution_stream_event_session_id(&event),
                        expected_kernel_generation
                            .map(|generation| generation.to_string())
                            .unwrap_or_else(|| String::from("<none>"))
                    ),
                );
                break StreamTaskExitReason::StaleKernelGeneration;
            }
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    match handle_prepared_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &request,
                        preparation,
                        session_id,
                        stream_task_started_at,
                        auto_compaction_already_attempted,
                        active_stream_task_id,
                    )
                    .await
                    {
                        PreparedStreamEventAction::Continue => {}
                        PreparedStreamEventAction::Restarted
                        | PreparedStreamEventAction::Failed => {
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    provider_family,
                    snapshot,
                } => {
                    if let Err(error) = record_webui_provider_request_checkpoint(
                        &controller_runtime,
                        session_id,
                        &provider_family,
                        &snapshot,
                    ) {
                        eprintln!("[webui-service] provider request checkpoint failed: {error}");
                    }
                }
                ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
                    provider_telemetry_guard.forward(event);
                }
                ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
                    if let Err(error) = record_webui_provider_lowering_repair(
                        &controller_runtime,
                        session_id,
                        event,
                    ) {
                        eprintln!("[webui-service] provider lowering repair failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestReady,
                    );
                }
                ExecutionStreamEvent::WireRequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::WireRequestReady,
                    );
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestStarted,
                    );
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Thinking,
                        delta,
                    );
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) = complete_streaming_thinking_phase(
                        &app,
                        &task_state,
                        session_id,
                        &record_ids,
                    ) {
                        eprintln!("[webui-service] complete thinking phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Output,
                        delta,
                    );
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    handle_runtime_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &mut request_trace_state,
                        &mut record_ids,
                        &runtime_event_hook_policy,
                        session_id,
                        active_stream_task_id,
                        event,
                    )
                    .await;
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    provider_request_count,
                } => handle_conversation_checkpoint_stream_event(
                    &app,
                    &task_state,
                    &controller_runtime,
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    provider_request_count,
                    request
                        .session_goal_context
                        .as_ref()
                        .map(|goal_context| goal_context.goal_id.as_str()),
                ),
                ExecutionStreamEvent::Finished(report) => {
                    match handle_finished_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        active_stream_task_id,
                        stream_task_started_at,
                        report,
                    )
                    .await
                    {
                        FinishedStreamEventOutcome::Handled
                        | FinishedStreamEventOutcome::StaleOwner
                        | FinishedStreamEventOutcome::OwnershipUnavailable => {
                            break StreamTaskExitReason::FinishedEventHandled;
                        }
                    }
                }
            }
        };
        // stream_exit_outcome records the exact ownership decision made after polling stops.
        // stream_exit_outcome 记录轮询停止后作出的精确所有权决策。
        let stream_exit_outcome = handle_stream_task_exit(
            &task_state,
            session_id,
            active_stream_task_id,
            stream_exit_reason,
        );
        webui_trace(
            "stream_task_exit_handled",
            format!(
                "session={session_id} reason={stream_exit_reason:?} outcome={stream_exit_outcome:?}"
            ),
        );
    };
    if let Err(error) = spawn_registered_active_stream_task(
        &state,
        session_id,
        active_stream_task_id,
        session_goal_id,
        task_future,
    ) {
        eprintln!("[webui-service] register stream task failed: {error}");
        match handle_stream_task_registration_failure(
            &registration_failure_app,
            &registration_failure_state,
            session_id,
            expected_kernel_generation,
            &error,
        ) {
            Ok(outcome) => webui_trace(
                "stream_task_registration_failure_handled",
                format!("session={session_id} outcome={outcome:?}"),
            ),
            Err(cleanup_error) => eprintln!(
                "[webui-service] register stream task failure projection failed: {cleanup_error}"
            ),
        }
    }
}
use content_delta::handle_content_delta_stream_event;
